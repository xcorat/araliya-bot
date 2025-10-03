#!/usr/bin/env python3
"""
Test script for verifying Araliya Bot deployment.
Run this script to validate the deployed API endpoints.
"""

import asyncio
import json
import sys
import time
from typing import Dict, Any
import httpx


class DeploymentTester:
    """Test suite for deployed Araliya Bot API."""
    
    def __init__(self, base_url: str):
        """Initialize tester with base URL."""
        self.base_url = base_url.rstrip('/')
        self.session_id = f"test-session-{int(time.time())}"
        self.client = httpx.AsyncClient(timeout=30.0)
    
    async def test_root_endpoint(self) -> Dict[str, Any]:
        """Test root endpoint."""
        print("Testing root endpoint...")
        try:
            response = await self.client.get(f"{self.base_url}/")
            result = {
                "endpoint": "GET /",
                "status_code": response.status_code,
                "success": response.status_code == 200,
                "response": response.json() if response.status_code == 200 else response.text
            }
            print(f"✓ Root endpoint: {response.status_code}")
            return result
        except Exception as e:
            print(f"✗ Root endpoint failed: {e}")
            return {"endpoint": "GET /", "success": False, "error": str(e)}
    
    async def test_health_endpoint(self) -> Dict[str, Any]:
        """Test health check endpoint."""
        print("Testing health endpoint...")
        try:
            response = await self.client.get(f"{self.base_url}/api/v1/health")
            result = {
                "endpoint": "GET /api/v1/health",
                "status_code": response.status_code,
                "success": response.status_code in [200, 503],  # 503 acceptable if OpenAI is down
                "response": response.json() if response.status_code in [200, 503] else response.text
            }
            
            if response.status_code == 200:
                print("✓ Health check: Healthy")
            elif response.status_code == 503:
                print("⚠ Health check: Degraded (OpenAI connectivity issue)")
            else:
                print(f"✗ Health check failed: {response.status_code}")
            
            return result
        except Exception as e:
            print(f"✗ Health endpoint failed: {e}")
            return {"endpoint": "GET /api/v1/health", "success": False, "error": str(e)}
    
    async def test_chat_endpoint(self) -> Dict[str, Any]:
        """Test chat endpoint."""
        print("Testing chat endpoint...")
        try:
            payload = {
                "message": "Hello! This is a test message.",
                "session_id": self.session_id
            }
            
            response = await self.client.post(
                f"{self.base_url}/api/v1/chat",
                json=payload,
                headers={"Content-Type": "application/json"}
            )
            
            result = {
                "endpoint": "POST /api/v1/chat",
                "status_code": response.status_code,
                "success": response.status_code == 200,
                "response": response.json() if response.status_code == 200 else response.text
            }
            
            if response.status_code == 200:
                data = response.json()
                print(f"✓ Chat endpoint: Response received")
                print(f"  Session ID: {data.get('session_id')}")
                print(f"  Response length: {len(data.get('message', ''))}")
            else:
                print(f"✗ Chat endpoint failed: {response.status_code}")
            
            return result
        except Exception as e:
            print(f"✗ Chat endpoint failed: {e}")
            return {"endpoint": "POST /api/v1/chat", "success": False, "error": str(e)}
    
    async def test_session_info_endpoint(self) -> Dict[str, Any]:
        """Test session info endpoint."""
        print("Testing session info endpoint...")
        try:
            response = await self.client.get(f"{self.base_url}/api/v1/sessions/{self.session_id}")
            result = {
                "endpoint": f"GET /api/v1/sessions/{self.session_id}",
                "status_code": response.status_code,
                "success": response.status_code == 200,
                "response": response.json() if response.status_code == 200 else response.text
            }
            
            if response.status_code == 200:
                data = response.json()
                print(f"✓ Session info: Found session")
                print(f"  Message count: {data.get('message_count', 0)}")
            else:
                print(f"✗ Session info failed: {response.status_code}")
            
            return result
        except Exception as e:
            print(f"✗ Session info endpoint failed: {e}")
            return {"endpoint": f"GET /api/v1/sessions/{self.session_id}", "success": False, "error": str(e)}
    
    async def test_conversation_flow(self) -> Dict[str, Any]:
        """Test multi-turn conversation."""
        print("Testing conversation flow...")
        try:
            messages = [
                "What is 2 + 2?",
                "Can you explain that calculation?",
                "Thank you for the explanation."
            ]
            
            responses = []
            for i, message in enumerate(messages):
                payload = {
                    "message": message,
                    "session_id": self.session_id
                }
                
                response = await self.client.post(
                    f"{self.base_url}/api/v1/chat",
                    json=payload,
                    headers={"Content-Type": "application/json"}
                )
                
                if response.status_code == 200:
                    data = response.json()
                    responses.append({
                        "message": message,
                        "response": data.get("message", ""),
                        "metadata": data.get("metadata", {})
                    })
                    print(f"  Turn {i+1}: ✓")
                else:
                    print(f"  Turn {i+1}: ✗ ({response.status_code})")
                    break
            
            result = {
                "endpoint": "Conversation Flow",
                "success": len(responses) == len(messages),
                "turns_completed": len(responses),
                "total_turns": len(messages),
                "responses": responses
            }
            
            if result["success"]:
                print("✓ Conversation flow: All turns completed")
            else:
                print(f"✗ Conversation flow: Only {len(responses)}/{len(messages)} turns completed")
            
            return result
        except Exception as e:
            print(f"✗ Conversation flow failed: {e}")
            return {"endpoint": "Conversation Flow", "success": False, "error": str(e)}
    
    async def test_error_handling(self) -> Dict[str, Any]:
        """Test error handling with invalid requests."""
        print("Testing error handling...")
        try:
            # Test empty message
            response = await self.client.post(
                f"{self.base_url}/api/v1/chat",
                json={"message": ""},
                headers={"Content-Type": "application/json"}
            )
            
            empty_message_test = {
                "test": "empty_message",
                "status_code": response.status_code,
                "success": response.status_code == 422  # Validation error expected
            }
            
            # Test missing message
            response = await self.client.post(
                f"{self.base_url}/api/v1/chat",
                json={},
                headers={"Content-Type": "application/json"}
            )
            
            missing_message_test = {
                "test": "missing_message",
                "status_code": response.status_code,
                "success": response.status_code == 422  # Validation error expected
            }
            
            # Test nonexistent session
            response = await self.client.get(f"{self.base_url}/api/v1/sessions/nonexistent-session")
            
            nonexistent_session_test = {
                "test": "nonexistent_session",
                "status_code": response.status_code,
                "success": response.status_code == 404  # Not found expected
            }
            
            tests = [empty_message_test, missing_message_test, nonexistent_session_test]
            success_count = sum(1 for test in tests if test["success"])
            
            result = {
                "endpoint": "Error Handling",
                "success": success_count == len(tests),
                "tests_passed": success_count,
                "total_tests": len(tests),
                "details": tests
            }
            
            print(f"✓ Error handling: {success_count}/{len(tests)} tests passed")
            return result
            
        except Exception as e:
            print(f"✗ Error handling tests failed: {e}")
            return {"endpoint": "Error Handling", "success": False, "error": str(e)}
    
    async def run_all_tests(self) -> Dict[str, Any]:
        """Run all deployment tests."""
        print(f"Starting deployment tests for: {self.base_url}")
        print("=" * 50)
        
        tests = [
            self.test_root_endpoint(),
            self.test_health_endpoint(),
            self.test_chat_endpoint(),
            self.test_session_info_endpoint(),
            self.test_conversation_flow(),
            self.test_error_handling()
        ]
        
        results = []
        for test in tests:
            result = await test
            results.append(result)
            print()  # Add spacing between tests
        
        # Summary
        successful_tests = sum(1 for result in results if result.get("success", False))
        total_tests = len(results)
        
        print("=" * 50)
        print(f"DEPLOYMENT TEST SUMMARY")
        print(f"Successful tests: {successful_tests}/{total_tests}")
        print(f"Success rate: {(successful_tests/total_tests)*100:.1f}%")
        
        if successful_tests == total_tests:
            print("🎉 All tests passed! Deployment is ready.")
        else:
            print("⚠️  Some tests failed. Check the results above.")
        
        await self.client.aclose()
        
        return {
            "summary": {
                "total_tests": total_tests,
                "successful_tests": successful_tests,
                "success_rate": (successful_tests/total_tests)*100,
                "overall_success": successful_tests == total_tests
            },
            "results": results
        }


async def main():
    """Main function to run deployment tests."""
    if len(sys.argv) != 2:
        print("Usage: python test_deployment.py <base_url>")
        print("Example: python test_deployment.py https://username-araliya-bot.hf.space")
        sys.exit(1)
    
    base_url = sys.argv[1]
    tester = DeploymentTester(base_url)
    
    try:
        results = await tester.run_all_tests()
        
        # Save results to file
        with open("deployment_test_results.json", "w") as f:
            json.dump(results, f, indent=2, default=str)
        
        print(f"\nDetailed results saved to: deployment_test_results.json")
        
        # Exit with appropriate code
        sys.exit(0 if results["summary"]["overall_success"] else 1)
        
    except Exception as e:
        print(f"Test execution failed: {e}")
        sys.exit(1)


if __name__ == "__main__":
    asyncio.run(main())
